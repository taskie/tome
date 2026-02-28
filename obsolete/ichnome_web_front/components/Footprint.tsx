import { IchFootprint } from "@/api/types";
import FootprintLink from "./FootprintLink";

type Props = {
  workspaceName: string;
  footprint: IchFootprint;
};

export const Footprint: React.FC<Props> = ({ workspaceName, footprint: { digest, size } }) => {
  return (
    <ul>
      <li>
        Digest: <FootprintLink workspaceName={workspaceName} digest={digest} />
      </li>
      <li>Size: {size}</li>
    </ul>
  );
};

export default Footprint;
