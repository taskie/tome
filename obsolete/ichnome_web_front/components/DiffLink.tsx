import Link from "next/link";
import { uri } from "@/utils/uri";

type Props = {
  workspaceName: string;
  groupName1: string;
  pathPrefix1: string;
  groupName2: string;
  pathPrefix2: string;
};

export const DiffLink: React.FC<Props> = ({
  workspaceName,
  groupName1,
  pathPrefix1,
  groupName2,
  pathPrefix2,
  children,
}) => {
  const query = {
    group_name1: groupName1,
    path_prefix1: pathPrefix1,
    group_name2: groupName2,
    path_prefix2: pathPrefix2,
  };
  const href = { pathname: "/[workspaceName]/diff", query };
  const as = { pathname: uri`/${workspaceName}/diff`, query };
  return (
    <Link href={href} as={as}>
      {children != null ? children : <a>Diff</a>}
    </Link>
  );
};

export default DiffLink;
