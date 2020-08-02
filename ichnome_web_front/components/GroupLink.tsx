import Link from "next/link";
import { uri } from "../utils/uri";

type Props = {
  workspaceName: string;
  groupName: string;
  family?: string;
  query?: { path_prefix?: string };
};

export const GroupLink: React.FC<Props> = ({ workspaceName, groupName, family, query, children }) => {
  const def =
    family === "groups"
      ? { href: "/[workspaceName]/groups/[groupName]", as: uri`/${workspaceName}/groups/${groupName}` }
      : { href: "/[workspaceName]/stats/[groupName]", as: uri`/${workspaceName}/stats/${groupName}` };
  const href = { pathname: def.href, query };
  const as = { pathname: def.as, query };
  return (
    <Link href={href} as={as}>
      {children != null ? children : <a>{groupName}</a>}
    </Link>
  );
};

export default GroupLink;
